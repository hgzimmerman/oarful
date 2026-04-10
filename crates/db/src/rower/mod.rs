pub mod queries;
pub mod types;

use types::{Height, RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength};

use crate::types::IntBool;

/// A rower on the team.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
    diesel::AsChangeset,
)]
#[diesel(table_name = crate::schema::rower)]
pub struct Rower {
    pub id: RowerId,
    pub name: String,
    /// Unique contact identifier. Populated by the Google Sheets sync
    /// and used as the matching key when re-importing. Nullable
    /// because not every rower has one on file (fixture rowers, legacy
    /// manual entries).
    pub email: Option<String>,
    pub weight_class: RowerWeightClass,
    pub skill: Skill,
    pub strength: Strength,
    /// Coarse height bucket, used by the S10 pair-height-balance soft
    /// constraint. Defaults to `Medium` for new/imported rowers.
    pub height: Height,
    pub side: Side,
    /// Side preference strength. `SideStrength::HARD` (= 0) means the
    /// rower is side-locked; 1..=5 are soft scales for the S4 penalty.
    pub side_strength: SideStrength,
    /// Eligible to be "pushed" to the scullers team as overflow. Does NOT
    /// affect sweep seating — this solver only assigns sweep seats.
    pub can_scull: IntBool,
    pub can_cox: IntBool,
    /// If true, this rower is a designated coxswain and is exempt from the
    /// cox-cooldown soft constraint.
    pub is_designated_cox: IntBool,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    /// FK to `app_user.id`. NULL until the rower claims their account.
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::rower)]
pub struct NewRower {
    pub name: String,
    pub email: Option<String>,
    pub weight_class: RowerWeightClass,
    pub skill: Skill,
    pub strength: Strength,
    pub height: Height,
    pub side: Side,
    pub side_strength: SideStrength,
    pub can_scull: IntBool,
    pub can_cox: IntBool,
    pub is_designated_cox: IntBool,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

impl NewRower {
    /// Convenience builder with sensible defaults for a new sweep rower.
    ///
    /// Most rowers at a typical club are trained to cox in a pinch,
    /// so `can_cox` defaults to `true`. Opt out by setting it to
    /// `IntBool::FALSE` for rowers who genuinely can't cox (e.g.,
    /// brand-new learn-to-row members). `is_designated_cox` stays
    /// `false` — designated coxes are the rare case and the admin
    /// sets it explicitly. Email is None by default; set it when
    /// creating rowers from a sync that has one.
    pub fn sweep(
        name: impl Into<String>,
        weight_class: RowerWeightClass,
        skill: Skill,
        strength: Strength,
        height: Height,
        side: Side,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            name: name.into(),
            email: None,
            weight_class,
            skill,
            strength,
            height,
            side,
            side_strength: SideStrength::default(),
            can_scull: IntBool::FALSE,
            can_cox: IntBool::TRUE,
            is_designated_cox: IntBool::FALSE,
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
        }
    }

    /// Builder used by the Google Sheets sync path. Applies
    /// middle-ground defaults for attributes the sheet doesn't
    /// know about (weight_class / skill / strength) and uses the
    /// sheet-provided values for everything it does know.
    pub fn from_sheet(
        name: impl Into<String>,
        email: String,
        side: Side,
        can_scull: bool,
        can_cox: bool,
        is_designated_cox: bool,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            name: name.into(),
            email: Some(email),
            weight_class: RowerWeightClass::Medium,
            skill: Skill::Intermediate,
            strength: Strength::Intermediate,
            height: Height::Medium,
            side,
            side_strength: SideStrength::default(),
            can_scull: IntBool::new(can_scull),
            can_cox: IntBool::new(can_cox),
            is_designated_cox: IntBool::new(is_designated_cox),
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
        }
    }
}
