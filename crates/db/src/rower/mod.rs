pub mod queries;
pub mod types;

use types::{RowerId, RowerWeightClass, Side, Skill, Strength};

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
    pub weight_class: RowerWeightClass,
    pub skill: Skill,
    pub strength: Strength,
    pub side: Side,
    /// 0 = hard constraint, 1..5 = soft preference strength.
    pub side_strength: i32,
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
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::rower)]
pub struct NewRower {
    pub name: String,
    pub weight_class: RowerWeightClass,
    pub skill: Skill,
    pub strength: Strength,
    pub side: Side,
    pub side_strength: i32,
    pub can_scull: IntBool,
    pub can_cox: IntBool,
    pub is_designated_cox: IntBool,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

impl NewRower {
    /// Convenience builder with sensible defaults for a new sweep rower.
    pub fn sweep(
        name: impl Into<String>,
        weight_class: RowerWeightClass,
        skill: Skill,
        strength: Strength,
        side: Side,
    ) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            name: name.into(),
            weight_class,
            skill,
            strength,
            side,
            side_strength: 3,
            can_scull: IntBool::FALSE,
            can_cox: IntBool::FALSE,
            is_designated_cox: IntBool::FALSE,
            active: IntBool::TRUE,
            created_at: now,
            updated_at: now,
        }
    }
}
