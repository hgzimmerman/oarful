//! Self-service club registration: creates a tenant + PD account.

use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    Form,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use lineup_db::app_user::{AppUser, NewAppUser, Role};
use lineup_db::team::{NewTeam, Team};
use lineup_master_db::tenant::{NewTenant, Tenant};
use serde::Deserialize;

use crate::{state::AppState, templates, templates::signup::SignupPrefill};

/// Trial period in days for new clubs.
const TRIAL_DAYS: i64 = 30;

#[derive(Debug, Deserialize)]
pub(crate) struct SignupInput {
    pub club_name: String,
    pub name: String,
    pub email: String,
    pub password: String,
    pub password_confirm: String,
}

/// `GET /signup` — render the registration form.
pub(crate) async fn signup_page() -> Html<String> {
    Html(templates::signup::signup_page(None, &SignupPrefill::default()).into_string())
}

/// `POST /signup` — validate, create tenant + PD user, issue JWT.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn signup_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<SignupInput>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let prefill = SignupPrefill {
        club_name: input.club_name.clone(),
        name: input.name.clone(),
        email: input.email.clone(),
    };

    // Validate input.
    let club_name = input.club_name.trim().to_string();
    if club_name.is_empty() || club_name.len() > 100 {
        return Ok(form_error(
            "Club name is required (max 100 characters).",
            &prefill,
        ));
    }
    let admin_name = input.name.trim().to_string();
    if admin_name.is_empty() {
        return Ok(form_error("Your name is required.", &prefill));
    }
    let email = input.email.trim().to_lowercase();
    if !email.contains('@') || !email.contains('.') {
        return Ok(form_error("Please enter a valid email address.", &prefill));
    }
    let password = input.password;
    if password.len() < 8 {
        return Ok(form_error(
            "Password must be at least 8 characters.",
            &prefill,
        ));
    }
    if password != input.password_confirm {
        return Ok(form_error("Passwords do not match.", &prefill));
    }

    // Generate slug from club name.
    let slug = slugify(&club_name);
    if slug.is_empty() {
        return Ok(form_error(
            "Club name must contain at least one letter or number.",
            &prefill,
        ));
    }

    // Ensure slug is unique, appending a suffix if needed.
    let slug = {
        let base = slug.clone();
        let mut candidate = base.clone();
        let mut i = 1u32;
        loop {
            let c = candidate.clone();
            let exists = state
                .master_db
                .with_conn(move |conn| Tenant::find_by_slug(conn, &c))
                .await
                .map_err(super::internal_error)?;
            if exists.is_none() {
                break candidate;
            }
            i += 1;
            candidate = format!("{base}-{i}");
        }
    };

    // Check email uniqueness across all tenants.
    let email_for_check = email.clone();
    let tenants = state
        .master_db
        .with_conn(|conn| Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    for t in &tenants {
        let (db, _config) = match state.tenant_db(t.id).await {
            Ok(pair) => pair,
            Err(_) => continue,
        };
        let email_clone = email_for_check.clone();
        let found = db
            .with_conn(move |conn| AppUser::find_by_email(conn, &email_clone))
            .await
            .map_err(super::internal_error)?;
        if found.is_some() {
            return Ok(form_error(
                "This email is already registered. Try signing in.",
                &prefill,
            ));
        }
    }

    let now = Utc::now().naive_utc();
    let trial_expires = now + chrono::TimeDelta::try_days(TRIAL_DAYS).unwrap();
    let db_path = format!("{}/tenants/{slug}.db", state.data_dir);

    // Create tenant in master DB.
    let slug_clone = slug.clone();
    let db_path_clone = db_path.clone();
    let club_name_clone = club_name.clone();
    let tenant = state
        .master_db
        .with_conn(move |conn| {
            Tenant::create(
                conn,
                NewTenant {
                    name: club_name_clone,
                    slug: slug_clone,
                    db_path: db_path_clone,
                    created_at: now,
                    billing_status: "trial".to_string(),
                    trial_expires_at: Some(trial_expires),
                },
            )
        })
        .await
        .map_err(super::internal_error)?;

    // Open the tenant DB (runs migrations on the fresh file).
    let (db, _config) = state
        .tenant_db(tenant.id)
        .await
        .map_err(super::internal_error)?;

    // Create a team + PD user inside the tenant DB.
    let club_name_for_team = club_name.clone();
    let (user, team, role) = db
        .with_conn(move |conn| {
            let team = Team::create(
                conn,
                NewTeam {
                    name: club_name_for_team,
                    created_at: now,
                },
            )?;

            let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
                .map_err(|e| diesel::result::Error::QueryBuilderError(Box::new(e)))?;

            let user = AppUser::create(
                conn,
                NewAppUser {
                    email,
                    password_hash: Some(hash),
                    name: admin_name,
                    status: "active".to_string(),
                    created_at: now,
                    updated_at: now,
                },
            )?;
            AppUser::set_role(conn, user.id, Role::ProgramDirector)?;

            Ok((user, team, Role::ProgramDirector))
        })
        .await
        .map_err(super::internal_error)?;

    // Issue JWT.
    let jwt = state
        .jwt_keys
        .issue(user.id, tenant.id, role, team.id)
        .map_err(super::internal_error)?;

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((super::auth::TOKEN_COOKIE, jwt))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    // Set known_user cookie so returning users get the magic-link option.
    let known_cookie = axum_extra::extract::cookie::Cookie::build(("known_user", user.email))
        .path("/")
        .http_only(true)
        .max_age(time::Duration::days(365))
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie).add(known_cookie);
    Ok((jar, Redirect::to("/practices")).into_response())
}

fn form_error(msg: &str, prefill: &SignupPrefill) -> axum::response::Response {
    Html(templates::signup::signup_page(Some(msg), prefill).into_string()).into_response()
}

/// Convert a club name into a URL-safe slug.
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
