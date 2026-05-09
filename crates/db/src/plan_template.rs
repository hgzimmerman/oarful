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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::in_memory_conn;
    use crate::timeline::Timeline;

    fn make_template(conn: &mut SqliteConnection, name: &str) -> PlanTemplate {
        PlanTemplate::create(
            conn,
            NewPlanTemplate {
                name: name.to_string(),
                description: String::new(),
                author_id: None,
                timeline_json: Timeline::default_empty(90).to_json(),
            },
        )
        .unwrap()
    }

    #[test]
    fn create_and_get() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "Race day");
        assert_eq!(t.name, "Race day");

        let found = PlanTemplate::get(&mut conn, t.id).unwrap().unwrap();
        assert_eq!(found.id, t.id);
        assert_eq!(found.name, "Race day");
    }

    #[test]
    fn list_ordered_by_name() {
        let mut conn = in_memory_conn();
        make_template(&mut conn, "Zebra");
        make_template(&mut conn, "Alpha");

        let list = PlanTemplate::list(&mut conn).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].name, "Zebra");
    }

    #[test]
    fn delete() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "Doomed");
        assert_eq!(PlanTemplate::delete(&mut conn, t.id).unwrap(), 1);
        assert!(PlanTemplate::get(&mut conn, t.id).unwrap().is_none());
    }

    #[test]
    fn update_meta() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "Old name");
        PlanTemplate::update_meta(&mut conn, t.id, "New name", "A description").unwrap();

        let updated = PlanTemplate::get(&mut conn, t.id).unwrap().unwrap();
        assert_eq!(updated.name, "New name");
        assert_eq!(updated.description, "A description");
    }

    #[test]
    fn update_timeline() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "T");
        let tl = t.timeline().unwrap();
        assert_eq!(tl.target_minutes, 90);

        let mut new_tl = tl;
        new_tl.target_minutes = 120;
        PlanTemplate::update_timeline(&mut conn, t.id, &new_tl).unwrap();

        let updated = PlanTemplate::get(&mut conn, t.id).unwrap().unwrap();
        assert_eq!(updated.timeline().unwrap().target_minutes, 120);
    }

    #[test]
    fn timeline_versioned_round_trip() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "V");
        // Stored as versioned JSON, readable back.
        let tl = t.timeline().unwrap();
        assert_eq!(tl.target_minutes, 90);
        assert!(t.timeline_json.contains("\"version\""));
    }

    #[test]
    fn duplicate_copies_categories() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "Original");
        set_categories_by_name(&mut conn, t.id, &["steady".to_string(), "race".to_string()])
            .unwrap();

        let dup = PlanTemplate::duplicate(&mut conn, t.id, "Copy".into()).unwrap();
        assert_eq!(dup.name, "Copy");

        let orig_cats = categories_for(&mut conn, t.id).unwrap();
        let dup_cats = categories_for(&mut conn, dup.id).unwrap();
        assert_eq!(orig_cats.len(), 2);
        assert_eq!(dup_cats.len(), 2);
        assert_eq!(
            orig_cats.iter().map(|c| &c.name).collect::<Vec<_>>(),
            dup_cats.iter().map(|c| &c.name).collect::<Vec<_>>(),
        );
    }

    // ── Category tests ───────────────────────────────────────────────

    #[test]
    fn get_or_create_is_idempotent() {
        let mut conn = in_memory_conn();
        let c1 = get_or_create_category(&mut conn, "Steady State").unwrap();
        let c2 = get_or_create_category(&mut conn, "steady state").unwrap();
        assert_eq!(c1.id, c2.id);
        assert_eq!(c1.name, "steady state");
    }

    #[test]
    fn set_categories_by_name_creates_and_assigns() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "T");
        let cats = set_categories_by_name(&mut conn, t.id, &["foo".into(), "bar".into()]).unwrap();
        assert_eq!(cats.len(), 2);

        let fetched = categories_for(&mut conn, t.id).unwrap();
        assert_eq!(fetched.len(), 2);
        let names: Vec<&str> = fetched.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
    }

    #[test]
    fn set_categories_replaces_previous() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "T");
        set_categories_by_name(&mut conn, t.id, &["old".into()]).unwrap();
        set_categories_by_name(&mut conn, t.id, &["new".into()]).unwrap();

        let cats = categories_for(&mut conn, t.id).unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "new");
    }

    #[test]
    fn set_categories_skips_empty_strings() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "T");
        let cats =
            set_categories_by_name(&mut conn, t.id, &["".into(), "  ".into(), "real".into()])
                .unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "real");
    }

    #[test]
    fn all_categories_lists_alphabetically() {
        let mut conn = in_memory_conn();
        get_or_create_category(&mut conn, "zzz").unwrap();
        get_or_create_category(&mut conn, "aaa").unwrap();

        let all = all_categories(&mut conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "aaa");
        assert_eq!(all[1].name, "zzz");
    }

    #[test]
    fn delete_template_cascades_category_links() {
        let mut conn = in_memory_conn();
        let t = make_template(&mut conn, "T");
        set_categories_by_name(&mut conn, t.id, &["cat".into()]).unwrap();
        PlanTemplate::delete(&mut conn, t.id).unwrap();

        // Category itself survives, just the link is gone.
        let all = all_categories(&mut conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "cat");
    }
}
