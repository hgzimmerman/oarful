//! Maud template functions. Each submodule exposes pure functions that
//! take borrowed data and return `Markup`. The base `page()` and navbar
//! live in [`layout`]; per-view content renderers live next to their
//! handler's domain.

pub(crate) mod attendance;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod billing;
pub(crate) mod boats;
pub(crate) mod confirm_modal;
pub(crate) mod email;
pub(crate) mod history;
pub(crate) mod landing;
pub(crate) mod layout;
pub(crate) mod my;
pub(crate) mod oar_sets;
pub(crate) mod onboarding;
pub(crate) mod plan_templates;
pub(crate) mod practices;
pub(crate) mod rowers;
pub(crate) mod signup;
pub(crate) mod solve;
pub(crate) mod superuser;
pub(crate) mod sync;
pub(crate) mod teams;
pub(crate) mod timeline;
pub(crate) mod unsubscribe;
pub(crate) mod users;
