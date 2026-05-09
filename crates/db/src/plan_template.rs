//! Saved practice plan templates — reusable full timelines.

use crate::app_user::UserId;
use crate::schema::{category, plan_template_category, practice_plan_template};
use crate::timeline::Timeline;
use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct PlanTemplateId(i32);

impl std::fmt::Display for PlanTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct CategoryId(i32);

impl std::fmt::Display for CategoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = practice_plan_template)]
pub struct PlanTemplate {
    pub id: PlanTemplateId,
    pub name: String,
    pub description: String,
    pub author_id: Option<UserId>,
    pub timeline_json: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = practice_plan_template)]
pub struct NewPlanTemplate {
    pub name: String,
    pub description: String,
    pub author_id: Option<UserId>,
    pub timeline_json: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = category)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = category)]
struct NewCategory {
    name: String,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = plan_template_category)]
struct PlanTemplateCategory {
    template_id: PlanTemplateId,
    category_id: CategoryId,
}

impl PlanTemplate {
    /// Parse the stored JSON into a Timeline.
    pub fn timeline(&self) -> Option<Timeline> {
        Timeline::from_json(&self.timeline_json)
    }

    /// List all templates, ordered by name.
    pub fn list(conn: &mut SqliteConnection) -> Result<Vec<PlanTemplate>, diesel::result::Error> {
        practice_plan_template::table
            .order(practice_plan_template::name.asc())
            .select(PlanTemplate::as_select())
            .get_results(conn)
    }

    /// Get a template by ID.
    pub fn get(
        conn: &mut SqliteConnection,
        id: PlanTemplateId,
    ) -> Result<Option<PlanTemplate>, diesel::result::Error> {
        practice_plan_template::table
            .find(id)
            .select(PlanTemplate::as_select())
            .first(conn)
            .optional()
    }

    /// Create a new template.
    pub fn create(
        conn: &mut SqliteConnection,
        new: NewPlanTemplate,
    ) -> Result<PlanTemplate, diesel::result::Error> {
        diesel::insert_into(practice_plan_template::table)
            .values(&new)
            .returning(PlanTemplate::as_returning())
            .get_result(conn)
    }

    /// Update the timeline JSON.
    pub fn update_timeline(
        conn: &mut SqliteConnection,
        id: PlanTemplateId,
        tl: &Timeline,
    ) -> Result<(), diesel::result::Error> {
        let json = tl.to_json();
        diesel::update(practice_plan_template::table.find(id))
            .set((
                practice_plan_template::timeline_json.eq(&json),
                practice_plan_template::updated_at.eq(diesel::dsl::now),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Update name and description.
    pub fn update_meta(
        conn: &mut SqliteConnection,
        id: PlanTemplateId,
        name: &str,
        description: &str,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(practice_plan_template::table.find(id))
            .set((
                practice_plan_template::name.eq(name),
                practice_plan_template::description.eq(description),
                practice_plan_template::updated_at.eq(diesel::dsl::now),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Delete a template.
    pub fn delete(
        conn: &mut SqliteConnection,
        id: PlanTemplateId,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(practice_plan_template::table.find(id)).execute(conn)
    }

    /// Duplicate a template with a new name (including its categories).
    pub fn duplicate(
        conn: &mut SqliteConnection,
        id: PlanTemplateId,
        new_name: String,
    ) -> Result<PlanTemplate, diesel::result::Error> {
        let original = Self::get(conn, id)?.ok_or(diesel::result::Error::NotFound)?;
        let cats = categories_for(conn, id)?;
        let new_tmpl = Self::create(
            conn,
            NewPlanTemplate {
                name: new_name,
                description: original.description,
                author_id: original.author_id,
                timeline_json: original.timeline_json,
            },
        )?;
        let cat_ids: Vec<CategoryId> = cats.iter().map(|c| c.id).collect();
        set_categories(conn, new_tmpl.id, &cat_ids)?;
        Ok(new_tmpl)
    }
}

// ── Category operations ──────────────────────────────────────────────

/// List all categories, ordered by name.
pub fn all_categories(conn: &mut SqliteConnection) -> Result<Vec<Category>, diesel::result::Error> {
    category::table
        .order(category::name.asc())
        .select(Category::as_select())
        .get_results(conn)
}

/// Get or create a category by name (case-insensitive, stored lowercase).
pub fn get_or_create_category(
    conn: &mut SqliteConnection,
    name: &str,
) -> Result<Category, diesel::result::Error> {
    let normalized = name.trim().to_lowercase();
    if let Some(existing) = category::table
        .filter(category::name.eq(&normalized))
        .select(Category::as_select())
        .first(conn)
        .optional()?
    {
        return Ok(existing);
    }
    diesel::insert_into(category::table)
        .values(NewCategory { name: normalized })
        .returning(Category::as_returning())
        .get_result(conn)
}

/// Get the categories for a template.
pub fn categories_for(
    conn: &mut SqliteConnection,
    template_id: PlanTemplateId,
) -> Result<Vec<Category>, diesel::result::Error> {
    plan_template_category::table
        .inner_join(category::table)
        .filter(plan_template_category::template_id.eq(template_id))
        .select(Category::as_select())
        .order(category::name.asc())
        .get_results(conn)
}

/// Replace all categories for a template.
pub fn set_categories(
    conn: &mut SqliteConnection,
    template_id: PlanTemplateId,
    category_ids: &[CategoryId],
) -> Result<(), diesel::result::Error> {
    diesel::delete(
        plan_template_category::table.filter(plan_template_category::template_id.eq(template_id)),
    )
    .execute(conn)?;
    for &cat_id in category_ids {
        diesel::insert_into(plan_template_category::table)
            .values(PlanTemplateCategory {
                template_id,
                category_id: cat_id,
            })
            .execute(conn)?;
    }
    Ok(())
}

/// Set categories by name strings. Creates any that don't exist.
pub fn set_categories_by_name(
    conn: &mut SqliteConnection,
    template_id: PlanTemplateId,
    names: &[String],
) -> Result<Vec<Category>, diesel::result::Error> {
    let mut cats = Vec::new();
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        cats.push(get_or_create_category(conn, trimmed)?);
    }
    let ids: Vec<CategoryId> = cats.iter().map(|c| c.id).collect();
    set_categories(conn, template_id, &ids)?;
    Ok(cats)
}
