//! Magic link tokens for passwordless email authentication.
//!
//! A magic link lets an email recipient click a URL to authenticate
//! without entering a password. The token is SHA-256 hashed before
//! storage; only the hash lives in the DB.

use crate::app_user::UserId;
use crate::schema::magic_link;
use crate::team::TeamId;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = magic_link)]
pub struct MagicLink {
    pub token_hash: String,
    pub user_id: UserId,
    pub redirect_path: String,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    /// Optional team context. When set, the JWT issued on magic-link
    /// auth will use this as `active_team_id` instead of the default.
    pub team_id: Option<TeamId>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = magic_link)]
pub struct NewMagicLink {
    pub token_hash: String,
    pub user_id: UserId,
    pub redirect_path: String,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub team_id: Option<TeamId>,
}

impl MagicLink {
    /// Look up a magic link by its token hash. Returns `None` if not
    /// found or expired.
    pub fn validate(
        conn: &mut SqliteConnection,
        hash: &str,
    ) -> Result<Option<MagicLink>, diesel::result::Error> {
        let row: Option<MagicLink> = magic_link::table
            .filter(magic_link::token_hash.eq(hash))
            .select(MagicLink::as_select())
            .first(conn)
            .optional()?;

        match row {
            Some(link) if link.expires_at > chrono::Utc::now().naive_utc() => Ok(Some(link)),
            Some(_) => {
                // Expired — clean it up.
                diesel::delete(magic_link::table.filter(magic_link::token_hash.eq(hash)))
                    .execute(conn)?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Consume a magic link after successful use. Deletes it so it
    /// can't be replayed.
    pub fn consume(conn: &mut SqliteConnection, hash: &str) -> Result<(), diesel::result::Error> {
        diesel::delete(magic_link::table.filter(magic_link::token_hash.eq(hash))).execute(conn)?;
        Ok(())
    }

    /// Insert a new magic link row.
    pub fn create(
        conn: &mut SqliteConnection,
        new: NewMagicLink,
    ) -> Result<MagicLink, diesel::result::Error> {
        diesel::insert_into(magic_link::table)
            .values(&new)
            .returning(MagicLink::as_returning())
            .get_result(conn)
    }

    /// Delete all expired magic links (housekeeping).
    pub fn cleanup_expired(conn: &mut SqliteConnection) -> Result<usize, diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::delete(magic_link::table.filter(magic_link::expires_at.lt(now))).execute(conn)
    }
}
