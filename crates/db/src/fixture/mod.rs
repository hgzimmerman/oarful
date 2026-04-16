//! Test/dev fixture data.
//!
//! - [`dev`] — 14 rowers, 3 boats, 1 practice (baseline tests + dev server)
//! - [`demo`] — 24 rowers, 6 boats, 3 practices (self-service demo)

mod dev;
mod demo;

pub use dev::{seed_if_empty, seed_fleet_only};
pub use demo::{seed_demo, DemoSeed};
