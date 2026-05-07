use crate::schema::{lineup, practice};
use crate::team::TeamId;
use crate::timeline::Timeline;
use crate::types::{DurationMinutes, IntBool};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Newtyped identifier for a `practice` row. Transparent wrapper over
/// `i32` with `diesel_derive_newtype::DieselNewType` doing the column
/// glue. Matches the `BoatId` / `RowerId` pattern.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct PracticeId(i32);

impl PracticeId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for PracticeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for PracticeId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::practice)]
pub struct Practice {
    pub id: PracticeId,
    pub team_id: TeamId,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub notes: Option<String>,
    pub cancelled: IntBool,
    /// Per-practice duration override. If None, falls back to the
    /// team's `default_practice_duration_minutes`.
    pub duration_minutes: Option<DurationMinutes>,
    /// JSON-serialized practice timeline / plan.
    pub timeline_json: Option<String>,
    /// Coach explicitly dismissed plan-building for this practice.
    pub plan_dismissed: IntBool,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::practice)]
pub struct NewPractice {
    pub team_id: TeamId,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub notes: Option<String>,
}

impl Practice {
    /// Look up a practice by its primary key.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: PracticeId,
    ) -> Result<Option<Practice>, diesel::result::Error> {
        practice::table
            .find(id)
            .select(Practice::as_select())
            .first(conn)
            .optional()
    }

    /// Find or create a practice for a (team, date, time) triple. If one
    /// already exists, returns it unchanged (notes are not overwritten).
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
        time: Option<NaiveTime>,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        let mut query = practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq(date))
            .into_boxed();
        if let Some(t) = time {
            query = query.filter(practice::time.eq(t));
        } else {
            query = query.filter(practice::time.is_null());
        }
        if let Some(existing) = query.select(Practice::as_select()).first(conn).optional()? {
            return Ok(existing);
        }
        diesel::insert_into(practice::table)
            .values(NewPractice {
                team_id,
                date,
                time,
                notes,
            })
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Practices with at least one committed (non-draft) lineup, newest first.
    /// Scoped to a single team.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_committed(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(
                practice::id.eq_any(
                    lineup::table
                        .filter(lineup::is_draft.eq(0))
                        .select(lineup::practice_id),
                ),
            )
            .select(Practice::as_select())
            .order(practice::date.desc())
            .get_results(conn)
    }

    /// Which of the given practice IDs have at least one committed (non-draft) lineup?
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn committed_ids(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        ids: &[PracticeId],
    ) -> Result<Vec<PracticeId>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::id.eq_any(ids))
            .filter(
                practice::id.eq_any(
                    lineup::table
                        .filter(lineup::is_draft.eq(0))
                        .select(lineup::practice_id),
                ),
            )
            .select(practice::id)
            .get_results(conn)
    }

    /// Update the notes on an existing practice row.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn update_notes_by_id(
        conn: &mut SqliteConnection,
        id: PracticeId,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(practice::table.find(id))
            .set(practice::notes.eq(notes))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Future practices (on or after `today`), ordered ascending.
    /// Excludes cancelled practices.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_upcoming(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        today: NaiveDate,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.ge(today))
            .filter(practice::cancelled.eq(0))
            .select(Practice::as_select())
            .order((practice::date.asc(), practice::time.asc()))
            .get_results(conn)
    }

    /// Toggle the cancelled flag on a practice by ID.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn set_cancelled_by_id(
        conn: &mut SqliteConnection,
        id: PracticeId,
        cancelled: bool,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(practice::table.find(id))
            .set(practice::cancelled.eq(if cancelled { 1 } else { 0 }))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Set or clear the plan_dismissed flag.
    pub fn set_plan_dismissed(
        conn: &mut SqliteConnection,
        id: PracticeId,
        dismissed: bool,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(practice::table.find(id))
            .set(practice::plan_dismissed.eq(if dismissed { 1 } else { 0 }))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Non-cancelled practices for a team on or after `since`,
    /// ordered ascending.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_since(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        since: NaiveDate,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.ge(since))
            .filter(practice::cancelled.eq(0))
            .select(Practice::as_select())
            .order((practice::date.asc(), practice::time.asc()))
            .get_results(conn)
    }

    /// Find an existing practice for a (team, date) pair.
    /// When multiple practices exist on the same date, returns the first.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn find_by_date(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
    ) -> Result<Option<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq(date))
            .select(Practice::as_select())
            .first(conn)
            .optional()
    }

    /// Display label for a practice. Shows just the date when alone,
    /// or date + time when `show_time` is true (multiple on same day).
    pub fn label(&self) -> String {
        match self.time {
            Some(t) => format!("{} · {}", self.date.format("%b %-d"), t.format("%-I:%M %p")),
            None => self.date.format("%b %-d").to_string(),
        }
    }

    /// Effective duration in minutes: per-practice override, then
    /// team default, then None (unknown).
    pub fn effective_duration(
        &self,
        team_default: Option<DurationMinutes>,
    ) -> Option<DurationMinutes> {
        self.duration_minutes.or(team_default)
    }

    /// Compute the [start, end) time window for this practice.
    /// Returns None if either time or duration is unknown.
    pub fn time_window(
        &self,
        team_default_duration: Option<DurationMinutes>,
    ) -> Option<(NaiveTime, NaiveTime)> {
        let start = self.time?;
        let dur = self.effective_duration(team_default_duration)?;
        let end = start + chrono::TimeDelta::minutes(dur.as_int() as i64);
        Some((start, end))
    }

    /// Find non-cancelled practices on *other* teams that overlap this
    /// practice's time window on the same date. Returns an empty vec if
    /// this practice has no time or duration set.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn find_overlapping(
        conn: &mut SqliteConnection,
        this: &Practice,
        this_team_default_duration: Option<DurationMinutes>,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        let Some((my_start, my_end)) = this.time_window(this_team_default_duration) else {
            return Ok(Vec::new());
        };
        // Candidate practices: same date, different team, not cancelled, has a time.
        let candidates: Vec<Practice> = practice::table
            .filter(practice::date.eq(this.date))
            .filter(practice::team_id.ne(this.team_id))
            .filter(practice::cancelled.eq(0))
            .filter(practice::time.is_not_null())
            .select(Practice::as_select())
            .get_results(conn)?;

        // Filter to those with overlapping time windows. We need each
        // candidate's team default duration — load lazily per team.
        use std::collections::HashMap;
        let mut team_defaults: HashMap<TeamId, Option<DurationMinutes>> = HashMap::new();
        let mut overlapping = Vec::new();
        for p in candidates {
            let team_dur = match team_defaults.get(&p.team_id) {
                Some(d) => *d,
                None => {
                    let t = crate::team::Team::get(conn, p.team_id)?;
                    let d = t.and_then(|t| t.default_practice_duration_minutes);
                    team_defaults.insert(p.team_id, d);
                    d
                }
            };
            if let Some((their_start, their_end)) = p.time_window(team_dur) {
                // Overlap: my_start < their_end AND their_start < my_end
                if my_start < their_end && their_start < my_end {
                    overlapping.push(p);
                }
            }
        }
        Ok(overlapping)
    }

    /// Parse the stored timeline JSON, if present.
    pub fn timeline(&self) -> Option<Timeline> {
        self.timeline_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    /// Save a timeline to this practice.
    #[tracing::instrument(level = "debug", skip(conn, timeline), err)]
    pub fn update_timeline(
        conn: &mut SqliteConnection,
        id: PracticeId,
        timeline: Option<&Timeline>,
    ) -> Result<Practice, diesel::result::Error> {
        let json = timeline.map(|t| serde_json::to_string(t).expect("timeline serializes"));
        diesel::update(practice::table.find(id))
            .set(practice::timeline_json.eq(json))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Full label including year.
    pub fn label_full(&self) -> String {
        match self.time {
            Some(t) => format!(
                "{} · {}",
                self.date.format("%A, %B %-d, %Y"),
                t.format("%-I:%M %p")
            ),
            None => self.date.format("%A, %B %-d, %Y").to_string(),
        }
    }
}

// ── Practice phase derivation ────────────────────────────────────────

/// The forward-moving lifecycle phase of a practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PracticePhase {
    /// Date/time set, collecting availability, no committed lineup yet.
    Created,
    /// Lineup committed, but no plan and plan not dismissed.
    Committed,
    /// Lineup committed + plan built (or plan dismissed). Ready to send.
    Ready,
    /// All boated rowers have been notified.
    Notified,
    /// Notified + practice end time has passed + not stale.
    Complete,
    /// Cancelled from any state.
    Cancelled,
}

impl PracticePhase {
    /// Human-readable label for display in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Created => "Needs lineup",
            Self::Committed => "Needs plan",
            Self::Ready => "Ready to send",
            Self::Notified => "Notified",
            Self::Complete => "Complete",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// Summary of a practice with its derived phase, used for list views.
#[derive(Debug, Clone)]
pub struct PracticeWithPhase {
    pub practice: Practice,
    pub phase: PracticePhase,
    pub is_stale: bool,
    pub boat_count: usize,
    pub boated_rower_count: usize,
    pub yes_count: usize,
    pub total_responses: usize,
    pub non_respondent_count: usize,
    pub unnotified_rower_count: usize,
}

/// Inputs needed to derive the phase for a single practice.
/// Gathered in batch by the list query and passed in per-practice.
pub struct PhaseDeriveInput {
    pub has_committed_lineup: bool,
    pub has_plan: bool,
    pub plan_dismissed: bool,
    pub is_stale: bool,
    pub all_boated_notified: bool,
    pub has_any_boated_rowers: bool,
    pub practice_ended: bool,
}

/// Derive the phase for a practice given pre-computed inputs.
pub fn derive_phase(practice: &Practice, input: &PhaseDeriveInput) -> PracticePhase {
    if practice.cancelled.as_bool() {
        return PracticePhase::Cancelled;
    }
    if !input.has_committed_lineup {
        return PracticePhase::Created;
    }
    // Has committed lineup from here on.
    if input.has_any_boated_rowers
        && input.all_boated_notified
        && !input.is_stale
        && input.practice_ended
    {
        return PracticePhase::Complete;
    }
    if input.has_any_boated_rowers && input.all_boated_notified {
        return PracticePhase::Notified;
    }
    if input.has_plan || input.plan_dismissed {
        return PracticePhase::Ready;
    }
    PracticePhase::Committed
}

/// Load all practices for a team and derive their phases in batch.
///
/// Returns practices sorted by date ascending. Includes:
/// - All non-cancelled upcoming practices (date >= today)
/// - All cancelled practices with date >= today (so coaches see them)
/// - Recently completed/past practices (date >= `history_since`)
///
/// All sub-queries (lineups, availability, notifications, staleness)
/// are batched to avoid N+1.
pub fn list_with_phases(
    conn: &mut SqliteConnection,
    team_id: TeamId,
    now: NaiveDateTime,
    history_since: NaiveDate,
) -> Result<Vec<PracticeWithPhase>, diesel::result::Error> {
    use crate::app_user::AppUser;
    use crate::availability::Availability;
    use crate::lineup::Lineup;
    use crate::lineup_notification::LineupNotification;
    use crate::rower::Rower;
    use crate::team::{Team, TeamMembership};

    let today = now.date();

    // Load all practices in the window (upcoming + recent history).
    let practices: Vec<Practice> = practice::table
        .filter(practice::team_id.eq(team_id))
        .filter(practice::date.ge(history_since))
        .select(Practice::as_select())
        .order((practice::date.asc(), practice::time.asc()))
        .get_results(conn)?;

    if practices.is_empty() {
        return Ok(Vec::new());
    }

    let pids: Vec<PracticeId> = practices.iter().map(|p| p.id).collect();

    // Team settings.
    let team = Team::get(conn, team_id)?;
    let default_duration = team
        .as_ref()
        .and_then(|t| t.default_practice_duration_minutes);
    let assume_available = team
        .as_ref()
        .map(|t| t.assume_available.as_bool())
        .unwrap_or(false);

    // Rowers on this team who have user accounts (for non-respondent count).
    let team_rower_ids = TeamMembership::rower_ids_for_team(conn, team_id)?;
    let active_rowers: HashSet<_> = Rower::list_active(conn)?
        .into_iter()
        .filter(|r| team_rower_ids.contains(&r.id))
        .map(|r| r.id)
        .collect();
    let rowers_with_users = AppUser::rower_ids_with_users(conn)?;
    let notifiable_rowers: HashSet<_> = active_rowers
        .iter()
        .filter(|id| rowers_with_users.contains(id))
        .copied()
        .collect();

    // Batch queries.
    let committed_rowers = Lineup::committed_rower_ids_for_practices(conn, &pids)?;
    let boat_counts = Lineup::committed_boat_counts(conn, &pids)?;
    let raw_avail = Availability::map_for_practices(conn, &pids)?;
    let notified_map = LineupNotification::notified_rowers_for_practices(conn, &pids)?;

    // Pre-partition availability by practice to avoid O(n*m) per-practice scan.
    let mut avail_by_practice: HashMap<
        PracticeId,
        HashMap<crate::rower::types::RowerId, crate::availability::types::AvailabilityStatus>,
    > = HashMap::new();
    for ((rid, pid), status) in raw_avail {
        avail_by_practice
            .entry(pid)
            .or_default()
            .insert(rid, status);
    }

    let mut results = Vec::with_capacity(practices.len());
    for practice in practices {
        let pid = practice.id;
        let empty_vec = Vec::new();
        let boated: &Vec<_> = committed_rowers.get(&pid).unwrap_or(&empty_vec);
        let boated_set: HashSet<_> = boated.iter().copied().collect();
        let notified = notified_map.get(&pid);
        let boat_count = boat_counts.get(&pid).copied().unwrap_or(0);
        let practice_avail = avail_by_practice.get(&pid);

        // Staleness: a boated rower whose availability is no longer Yes.
        // Respects assume_available: if false, no response = not available = stale.
        let is_stale = !boated_set.is_empty()
            && boated_set.iter().any(|rid| {
                !practice_avail
                    .and_then(|m| m.get(rid))
                    .map(|s| s.is_available())
                    .unwrap_or(assume_available)
            });

        let practice_avail = practice_avail.cloned().unwrap_or_default();
        let yes_count = practice_avail.values().filter(|s| s.is_available()).count();
        let total_responses = practice_avail.len();
        let non_respondent_count = notifiable_rowers
            .iter()
            .filter(|id| !practice_avail.contains_key(id))
            .count();

        // Notification status.
        let all_boated_notified = if boated_set.is_empty() {
            false
        } else {
            match notified {
                Some(set) => boated_set.iter().all(|rid| set.contains(rid)),
                None => false,
            }
        };
        let unnotified_rower_count = if boated_set.is_empty() {
            0
        } else {
            match notified {
                Some(set) => boated_set.iter().filter(|rid| !set.contains(rid)).count(),
                None => boated_set.len(),
            }
        };

        // Has the practice ended?
        let practice_ended = if let Some((_, end_time)) = practice.time_window(default_duration) {
            let end_dt = practice.date.and_time(end_time);
            now > end_dt
        } else {
            // No time info — use end of day.
            today > practice.date
        };

        let has_plan = practice.timeline_json.is_some();
        let plan_dismissed = practice.plan_dismissed.as_bool();

        let input = PhaseDeriveInput {
            has_committed_lineup: !boated_set.is_empty() || boat_count > 0,
            has_plan,
            plan_dismissed,
            is_stale,
            all_boated_notified,
            has_any_boated_rowers: !boated_set.is_empty(),
            practice_ended,
        };
        let phase = derive_phase(&practice, &input);

        results.push(PracticeWithPhase {
            practice,
            phase,
            is_stale,
            boat_count,
            boated_rower_count: boated_set.len(),
            yes_count,
            total_responses,
            non_respondent_count,
            unnotified_rower_count,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::{NewTeam, Team};
    use crate::test_support::in_memory_conn;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(
            conn,
            NewTeam {
                name: "Test".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn upsert_creates_new() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        assert_eq!(p.date, d(2026, 5, 1));
        assert_eq!(p.team_id, tid);
    }

    #[test]
    fn upsert_returns_existing() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p1 =
            Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, Some("note".into())).unwrap();
        let p2 = Practice::upsert(
            &mut conn,
            tid,
            d(2026, 5, 1),
            None,
            Some("different".into()),
        )
        .unwrap();
        assert_eq!(p1.id, p2.id);
        assert_eq!(p2.notes.as_deref(), Some("note")); // not overwritten
    }

    #[test]
    fn get() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        let fetched = Practice::get(&mut conn, p.id).unwrap().unwrap();
        assert_eq!(fetched.id, p.id);
        assert!(Practice::get(&mut conn, PracticeId::new(9999))
            .unwrap()
            .is_none());
    }

    #[test]
    fn update_notes() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        assert!(p.notes.is_none());

        let updated =
            Practice::update_notes_by_id(&mut conn, p.id, Some("new note".into())).unwrap();
        assert_eq!(updated.notes.as_deref(), Some("new note"));
    }

    #[test]
    fn list_upcoming_excludes_cancelled_and_past() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let today = d(2026, 5, 5);
        Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap(); // past
        Practice::upsert(&mut conn, tid, d(2026, 5, 5), None, None).unwrap(); // today
        Practice::upsert(&mut conn, tid, d(2026, 5, 10), None, None).unwrap(); // future
        let cancelled = Practice::upsert(&mut conn, tid, d(2026, 5, 15), None, None).unwrap();
        Practice::set_cancelled_by_id(&mut conn, cancelled.id, true).unwrap();

        let upcoming = Practice::list_upcoming(&mut conn, tid, today).unwrap();
        assert_eq!(upcoming.len(), 2); // today + future, not past or cancelled
        assert_eq!(upcoming[0].date, d(2026, 5, 5));
        assert_eq!(upcoming[1].date, d(2026, 5, 10));
    }

    #[test]
    fn set_cancelled_toggles() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        assert!(!p.cancelled.as_bool());

        let c = Practice::set_cancelled_by_id(&mut conn, p.id, true).unwrap();
        assert!(c.cancelled.as_bool());

        let c = Practice::set_cancelled_by_id(&mut conn, p.id, false).unwrap();
        assert!(!c.cancelled.as_bool());
    }

    #[test]
    fn find_by_date() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();

        assert!(Practice::find_by_date(&mut conn, tid, d(2026, 5, 1))
            .unwrap()
            .is_some());
        assert!(Practice::find_by_date(&mut conn, tid, d(2026, 5, 2))
            .unwrap()
            .is_none());
    }

    #[test]
    fn label_without_time() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        assert_eq!(p.label(), "May 1");
    }

    #[test]
    fn label_with_time() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let time = NaiveTime::from_hms_opt(6, 30, 0);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), time, None).unwrap();
        assert_eq!(p.label(), "May 1 · 6:30 AM");
    }

    #[test]
    fn effective_duration_prefers_practice_override() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let mut p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        p.duration_minutes = Some(DurationMinutes::new(45));

        let team_default = Some(DurationMinutes::new(90));
        assert_eq!(p.effective_duration(team_default).unwrap().as_int(), 45);
    }

    #[test]
    fn effective_duration_falls_back_to_team() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), None, None).unwrap();
        assert!(p.duration_minutes.is_none());

        let team_default = Some(DurationMinutes::new(90));
        assert_eq!(p.effective_duration(team_default).unwrap().as_int(), 90);
    }

    #[test]
    fn time_window() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let time = NaiveTime::from_hms_opt(6, 0, 0);
        let mut p = Practice::upsert(&mut conn, tid, d(2026, 5, 1), time, None).unwrap();
        p.duration_minutes = Some(DurationMinutes::new(90));

        let (start, end) = p.time_window(None).unwrap();
        assert_eq!(start, NaiveTime::from_hms_opt(6, 0, 0).unwrap());
        assert_eq!(end, NaiveTime::from_hms_opt(7, 30, 0).unwrap());
    }

    // ── Phase derivation tests ──────────────────────────────────────

    fn make_practice(conn: &mut SqliteConnection) -> Practice {
        let tid = seed_team(conn);
        Practice::upsert(conn, tid, d(2026, 5, 10), None, None).unwrap()
    }

    fn base_input() -> PhaseDeriveInput {
        PhaseDeriveInput {
            has_committed_lineup: false,
            has_plan: false,
            plan_dismissed: false,
            is_stale: false,
            all_boated_notified: false,
            has_any_boated_rowers: false,
            practice_ended: false,
        }
    }

    #[test]
    fn phase_created_when_no_lineup() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        assert_eq!(derive_phase(&p, &base_input()), PracticePhase::Created);
    }

    #[test]
    fn phase_committed_when_lineup_no_plan() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Committed);
    }

    #[test]
    fn phase_ready_when_plan_exists() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            has_plan: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Ready);
    }

    #[test]
    fn phase_ready_when_plan_dismissed() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            plan_dismissed: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Ready);
    }

    #[test]
    fn phase_notified_when_all_notified() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            has_plan: true,
            all_boated_notified: true,
            has_any_boated_rowers: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Notified);
    }

    #[test]
    fn phase_complete_when_ended_and_notified() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            has_plan: true,
            all_boated_notified: true,
            has_any_boated_rowers: true,
            practice_ended: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Complete);
    }

    #[test]
    fn phase_notified_not_complete_when_stale() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            has_plan: true,
            all_boated_notified: true,
            has_any_boated_rowers: true,
            practice_ended: true,
            is_stale: true,
            ..base_input()
        };
        // Stale prevents Complete, falls to Notified
        assert_eq!(derive_phase(&p, &input), PracticePhase::Notified);
    }

    #[test]
    fn phase_cancelled_overrides_all() {
        let mut conn = in_memory_conn();
        let p = make_practice(&mut conn);
        Practice::set_cancelled_by_id(&mut conn, p.id, true).unwrap();
        let p = Practice::get(&mut conn, p.id).unwrap().unwrap();
        let input = PhaseDeriveInput {
            has_committed_lineup: true,
            has_plan: true,
            all_boated_notified: true,
            has_any_boated_rowers: true,
            ..base_input()
        };
        assert_eq!(derive_phase(&p, &input), PracticePhase::Cancelled);
    }
}
