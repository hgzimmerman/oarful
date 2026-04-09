//! Database layer for lineup_generator.
//!
//! Mirrors the module-per-entity pattern used in the sibling `boat_tracking`
//! project: each entity gets a `mod.rs` with its struct defs, a `types.rs`
//! holding newtype IDs and `DbEnum`s, and a `queries.rs` with inherent-impl
//! query functions that take `&mut SqliteConnection`.

pub mod availability;
pub mod boat;
pub mod fixture;
pub mod lineup;
pub mod pair_affinity;
pub mod practice;
pub mod rower;
pub mod schema;
pub mod seat_affinity;
pub mod snapshot;
pub mod state;
pub mod types;

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

/// Embedded migrations, applied automatically by [`state::Db::connect`].
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

/// Re-export of the diesel sql_type glue the generated schema references.
/// The `diesel::table!` macro looks for `DbEnum` mappings under
/// `crate::sql_types::*` by convention.
pub mod sql_types {
    pub use super::availability::types::AvailabilityStatusMapping;
    pub use super::boat::types::WeightClassMapping;
    pub use super::rower::types::{
        HeightMapping, RowerWeightClassMapping, SideMapping, SkillMapping, StrengthMapping,
    };
}
