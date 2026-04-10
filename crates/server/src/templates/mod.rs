//! Maud template functions. Each submodule exposes pure functions that
//! take borrowed data and return `Markup`. The base `page()` and navbar
//! live in [`layout`]; per-view content renderers live next to their
//! handler's domain.

pub(crate) mod boats;
pub(crate) mod history;
pub(crate) mod layout;
pub(crate) mod practices;
pub(crate) mod rowers;
pub(crate) mod solve;
pub(crate) mod sync;
pub(crate) mod teams;
