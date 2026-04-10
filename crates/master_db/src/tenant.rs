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
