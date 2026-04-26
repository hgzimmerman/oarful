//! `GET /solve/{id}` — main solve view handler.

use axum::{extract::Path, response::Html, Extension};
use axum_extra::extract::{CookieJar, Query};
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::snapshot::DbSnapshot;

use crate::templates;

use super::*;

/// Tab metadata parsed from the `editor_tabs` cookie. Passed to
/// templates so the tab bar can be server-rendered.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct EditorTabsMeta {
    #[serde(default)]
    pub(crate) tabs: Vec<TabEntry>,
    #[serde(default)]
    pub(crate) active: i32,
    #[serde(default = "default_next_id")]
    #[serde(rename = "nextId")]
    pub(crate) next_id: i32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct TabEntry {
    pub(crate) id: i32,
    pub(crate) label: String,
}

fn default_next_id() -> i32 {
    1
}

impl Default for EditorTabsMeta {
    fn default() -> Self {
        Self {
            tabs: vec![TabEntry {
                id: 0,
                label: "Lineup 1".into(),
            }],
            active: 0,
            next_id: 1,
        }
    }
}

fn parse_tab_cookies(jar: &CookieJar) -> (EditorTabsMeta, String) {
    let meta: EditorTabsMeta = jar
        .get("editor_tabs")
        .and_then(|c| serde_json::from_str(c.value()).ok())
        .unwrap_or_default();
    let active_state = jar
        .get(&format!("tab_{}", meta.active))
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    (meta, active_state)
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn view_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Query(knobs): Query<SolveKnobs>,
    hx: HxRequest,
) -> Result<Html<String>, super::ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (practice, mut snapshot, committed_practices, has_committed, draft_lineups) = tenant
        .db
        .with_conn(move |conn| {
            let practice =
                Practice::get(conn, practice_id)?.ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            let practices = Practice::list_committed(conn, team_id)?;
            let has_committed = {
                use lineup_db::lineup::Lineup;
                Lineup::for_practice(conn, practice.id)
                    .map(|l| !l.is_empty())
                    .unwrap_or(false)
            };
            let drafts = lineup_db::lineup::Lineup::draft_for_practice(conn, practice.id)?;
            Ok((practice, snapshot, practices, has_committed, drafts))
        })
        .await
        .map_err(internal_error)?;

    let date = practice.date;

    // Apply walk-on overrides before anything reads availability.
    apply_walkons(&mut snapshot, &knobs);

    // Load custom solver profiles for this team.
    let custom_profiles = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::list_for_team(conn, team_id)
        })
        .await
        .map_err(internal_error)?;

    // Apply no-shows before anything reads availability — affects
    // both the editor pool and the solver.
    apply_no_shows(&mut snapshot, &knobs);

    // When generate=1, return the streaming skeleton. For HTMX
    // requests (form submit), swap just #solve-results. For direct
    // navigation (browser URL), wrap in the full page with knobs.
    if knobs.generate > 0 {
        let skeleton = templates::solve::streaming_skeleton(practice_id, &knobs);
        if hx.0 {
            return Ok(Html(skeleton.into_string()));
        }
        let profile_names: Vec<(String, Option<String>)> = custom_profiles
            .iter()
            .map(|p| (p.name.clone(), p.description.clone()))
            .collect();
        let content = templates::solve::streaming_page(
            &snapshot,
            practice_id,
            date,
            &knobs,
            &committed_practices,
            &profile_names,
        );
        return Ok(crate::handlers::maybe_page_authed(
            &format!("Set Lineups · {date}"),
            content,
            hx,
            &tenant,
        ));
    }

    // Load team boat defaults. For single-team tenants with no
    // defaults configured, all boats remain active (empty set).
    let default_boats: std::collections::HashSet<lineup_db::boat::types::BoatId> = {
        let team_count = tenant
            .db
            .with_conn(|conn| lineup_db::team::Team::list_all(conn).map(|t| t.len()))
            .await
            .map_err(internal_error)?;
        let defaults = tenant
            .db
            .with_conn(move |conn| {
                lineup_db::team::TeamBoatDefault::boat_ids_for_team(conn, team_id)
            })
            .await
            .map_err(internal_error)?;
        // Single-team tenant with no defaults → all boats (empty set signals "all").
        // Multi-team tenant: use whatever is configured (even if empty → none pre-selected).
        if team_count <= 1 && defaults.is_empty() {
            std::collections::HashSet::new()
        } else {
            defaults.into_iter().collect()
        }
    };

    // Parse tab cookies — active tab state overrides URL params and drafts.
    let (tab_meta, tab_state) = parse_tab_cookies(&jar);
    let has_tabs_with_content = tab_meta.tabs.iter().any(|t| {
        !jar.get(&format!("tab_{}", t.id))
            .map(|c| c.value().is_empty())
            .unwrap_or(true)
    });

    // If the active tab cookie has seat data, parse it into knobs-style
    // overrides so the editor loads from tab state.
    let effective_knobs = if !tab_state.is_empty() {
        // Parse the tab state (gatherState format) into a SolveKnobs-like
        // struct to extract seat and boat params.
        serde_html_form::from_str::<SolveKnobs>(&tab_state).unwrap_or_else(|_| knobs.clone())
    } else {
        knobs.clone()
    };

    // Landing page: show knobs + "Generate" / "Re-generate" button.
    let profile_names: Vec<(String, Option<String>)> = custom_profiles
        .iter()
        .map(|p| (p.name.clone(), p.description.clone()))
        .collect();
    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats: SolveKnobs::triples_to_set(&effective_knobs.lock),
        pinned_seats: SolveKnobs::triples_to_set(&effective_knobs.pin),
        was_pinned_seats: SolveKnobs::triples_to_set(&effective_knobs.was_pin),
        pinned_boats: SolveKnobs::boat_id_set(&effective_knobs.boat_pin),
        was_pinned_boats: SolveKnobs::boat_id_set(&effective_knobs.boat_was_pin),
        locked_boats: SolveKnobs::boat_id_set(&effective_knobs.boat_lock),
        boats_in_use_by: std::collections::HashMap::new(),
    };
    let content = templates::solve::landing_content(
        &snapshot,
        practice_id,
        date,
        &effective_knobs,
        &committed_practices,
        has_committed,
        &profile_names,
        &flags,
        &default_boats,
        &draft_lineups,
        &tab_meta,
        has_tabs_with_content,
    );
    Ok(crate::handlers::maybe_page_authed(
        &format!("Set Lineups · {date}"),
        content,
        hx,
        &tenant,
    ))
}
