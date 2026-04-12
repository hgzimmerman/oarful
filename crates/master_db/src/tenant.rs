//! Tenant: a rowing club. Each tenant gets its own SQLite file
//! containing the full domain schema (rowers, boats, teams, etc.).

use crate::schema::tenant;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

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
pub struct TenantId(i32);

impl TenantId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TenantId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::tenant)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub slug: String,
    pub db_path: String,
    pub created_at: NaiveDateTime,
    /// Whether rower attributes (weight class, skill, strength) are
    /// visible to all members. `0` = Coach+ only (default), `1` = public.
    pub attributes_public: i32,
    /// When `1`, always display the coxswain at the top of the lineup
    /// (stern position) regardless of per-boat `cox_position`.
    pub force_cox_stern: i32,
    /// When set, this tenant is an ephemeral demo that should be
    /// cleaned up after this timestamp.
    pub demo_expires_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::tenant)]
pub struct NewTenant {
    pub name: String,
    pub slug: String,
    pub db_path: String,
    pub created_at: NaiveDateTime,
}

impl Tenant {
    pub fn are_attributes_public(&self) -> bool {
        self.attributes_public != 0
    }

    pub fn force_cox_stern(&self) -> bool {
        self.force_cox_stern != 0
    }

    pub fn is_demo(&self) -> bool {
        self.demo_expires_at.is_some()
    }

    /// List all expired demo tenants (for cleanup).
    pub fn list_expired_demos(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Tenant>, diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        tenant::table
            .filter(tenant::demo_expires_at.is_not_null())
            .filter(tenant::demo_expires_at.lt(now))
            .select(Tenant::as_select())
            .get_results(conn)
    }

    /// Delete a tenant row from the master DB.
    pub fn delete(
        conn: &mut SqliteConnection,
        id: TenantId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(tenant::table.find(id)).execute(conn)?;
        Ok(())
    }

    pub fn list_all(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Tenant>, diesel::result::Error> {
        tenant::table
            .select(Tenant::as_select())
            .order(tenant::name.asc())
            .get_results(conn)
    }

    pub fn find_by_slug(
        conn: &mut SqliteConnection,
        slug: &str,
    ) -> Result<Option<Tenant>, diesel::result::Error> {
        tenant::table
            .filter(tenant::slug.eq(slug))
            .select(Tenant::as_select())
            .first(conn)
            .optional()
    }

    pub fn get(
        conn: &mut SqliteConnection,
        id: TenantId,
    ) -> Result<Option<Tenant>, diesel::result::Error> {
        tenant::table
            .find(id)
            .select(Tenant::as_select())
            .first(conn)
            .optional()
    }

    pub fn create(
        conn: &mut SqliteConnection,
        new: NewTenant,
    ) -> Result<Tenant, diesel::result::Error> {
        diesel::insert_into(tenant::table)
            .values(new)
            .returning(Tenant::as_returning())
            .get_result(conn)
    }
}
