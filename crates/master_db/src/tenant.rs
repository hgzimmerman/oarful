//! Tenant: a rowing club. Each tenant gets its own SQLite file
//! containing the full domain schema (rowers, boats, teams, etc.).

use crate::schema::tenant;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "snake_case"]
#[serde(rename_all = "snake_case")]
pub enum BillingStatus {
    /// Default for new signups — full app access, email blocked.
    Free,
    /// Paid — email enabled.
    Active,
    /// Permanently free + email — early adopters / beta testers.
    Grandfathered,
}

impl BillingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Active => "active",
            Self::Grandfathered => "grandfathered",
        }
    }

    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "grandfathered" => Self::Grandfathered,
            _ => Self::Free,
        }
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
    /// Whether email addresses are visible to members on the roster
    /// and rower detail pages. `0` = Coach+ only (default), `1` = visible.
    pub emails_visible: i32,
    /// Billing status: Free (email blocked), Active (paid), or
    /// Grandfathered (free + email).
    pub billing_status: BillingStatus,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::tenant)]
pub struct NewTenant {
    pub name: String,
    pub slug: String,
    pub db_path: String,
    pub created_at: NaiveDateTime,
    pub billing_status: BillingStatus,
}

impl Tenant {
    pub fn are_attributes_public(&self) -> bool {
        self.attributes_public != 0
    }

    pub fn are_emails_visible(&self) -> bool {
        self.emails_visible != 0
    }

    pub fn force_cox_stern(&self) -> bool {
        self.force_cox_stern != 0
    }

    pub fn is_demo(&self) -> bool {
        self.demo_expires_at.is_some()
    }

    /// Whether the tenant can access the app. Always true — billing
    /// only gates email, not app access. Demo tenants have their own
    /// expiry logic handled separately.
    pub fn is_billing_ok(&self) -> bool {
        true
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
    pub fn delete(conn: &mut SqliteConnection, id: TenantId) -> Result<(), diesel::result::Error> {
        diesel::delete(tenant::table.find(id)).execute(conn)?;
        Ok(())
    }

    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Tenant>, diesel::result::Error> {
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

    pub fn set_billing_status(
        conn: &mut SqliteConnection,
        id: TenantId,
        status: BillingStatus,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(tenant::table.find(id))
            .set(tenant::billing_status.eq(status))
            .execute(conn)?;
        Ok(())
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

    pub fn set_stripe_ids(
        conn: &mut SqliteConnection,
        id: TenantId,
        customer_id: &str,
        subscription_id: Option<&str>,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(tenant::table.find(id))
            .set((
                tenant::stripe_customer_id.eq(Some(customer_id)),
                tenant::stripe_subscription_id.eq(subscription_id),
            ))
            .execute(conn)?;
        Ok(())
    }

    pub fn find_by_stripe_customer_id(
        conn: &mut SqliteConnection,
        customer_id: &str,
    ) -> Result<Option<Tenant>, diesel::result::Error> {
        tenant::table
            .filter(tenant::stripe_customer_id.eq(customer_id))
            .select(Tenant::as_select())
            .first(conn)
            .optional()
    }

    pub fn clear_stripe_subscription(
        conn: &mut SqliteConnection,
        id: TenantId,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(tenant::table.find(id))
            .set(tenant::stripe_subscription_id.eq(None::<String>))
            .execute(conn)?;
        Ok(())
    }
}
