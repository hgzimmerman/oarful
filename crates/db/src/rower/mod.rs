pub mod queries;
pub mod types;

use types::{Height, RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias};

use crate::types::{HeightM, IntBool, WeightKg};

/// A rower on the team.
#[derive(
    Debug,
    Clone,
    PartialEq,
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
    /// How strongly this rower prefers sweep vs scull. -2 = hard sculler
    /// (excluded from sweep solve), 2 = sweep only (never pushed to scull).
    pub sweep_bias: SweepBias,
    pub can_cox: IntBool,
    /// If true, this rower is a designated coxswain and is exempt from the
    /// cox-cooldown soft constraint.
    pub is_designated_cox: IntBool,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    /// Body weight in kilograms.
    pub weight_kg: Option<WeightKg>,
    /// Height in metres.
    pub height_m: Option<HeightM>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

impl Rower {
    /// Preferred display name. Uses first + last when available,
    /// falls back to the legacy `name` column for unsplit records.
    pub fn display_name(&self) -> String {
        match (self.first_name.as_deref(), self.last_name.as_deref()) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            (Some(first), None) => first.to_string(),
            (None, Some(last)) => last.to_string(),
            (None, None) => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::rower)]
pub struct NewRower {
    pub name: String,
    pub weight_class: RowerWeightClass,
    pub skill: Skill,
    pub strength: Strength,
    pub height: Height,
    pub side: Side,
    pub side_strength: SideStrength,
    pub sweep_bias: SweepBias,
    pub can_cox: IntBool,
    pub is_designated_cox: IntBool,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

impl NewRower {
    /// Convenience builder with sensible defaults for a new sweep rower.
    ///
    /// Most rowers at a typical club are trained to cox in a pinch,
    /// so `can_cox` defaults to `true`. Opt out by setting it to
    /// `IntBool::FALSE` for rowers who genuinely can't cox (e.g.,
    /// brand-new learn-to-row members). `is_designated_cox` stays
    /// `false` — designated coxes are the rare case and the admin
    /// sets it explicitly.
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
            weight_class,
            skill,
            strength,
            height,
            side,
            side_strength: SideStrength::default(),
            sweep_bias: SweepBias::SWEEP_HARD,
            can_cox: IntBool::TRUE,
            is_designated_cox: IntBool::FALSE,
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
            first_name: None,
            last_name: None,
        }
    }

    /// Builder used by the Google Sheets sync path. Applies
    /// middle-ground defaults for attributes the sheet doesn't
    /// know about (weight_class / skill / strength) and uses the
    /// sheet-provided values for everything it does know.
    pub fn from_sheet(
        name: impl Into<String>,
        first_name: Option<String>,
        last_name: Option<String>,
        side: Side,
        sweep_bias: SweepBias,
        can_cox: bool,
        is_designated_cox: bool,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            name: name.into(),
            first_name,
            last_name,
            weight_class: RowerWeightClass::Medium,
            skill: Skill::Intermediate,
            strength: Strength::Intermediate,
            height: Height::Medium,
            side,
            side_strength: SideStrength::default(),
            sweep_bias,
            can_cox: IntBool::new(can_cox),
            is_designated_cox: IntBool::new(is_designated_cox),
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
        }
    }
}
