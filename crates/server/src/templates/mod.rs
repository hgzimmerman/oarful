//! Maud template functions. Each submodule exposes pure functions that
//! take borrowed data and return `Markup`. The base `page()` and navbar
//! live in [`layout`]; per-view content renderers live next to their
//! handler's domain.

pub mod history;
pub mod layout;
pub mod practices;
pub mod rowers;
pub mod solve;
