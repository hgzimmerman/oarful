//! Custom axum extractors.

use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
};
use serde::de::DeserializeOwned;

/// Form extractor that uses `serde_html_form` (same parser as
/// `axum_extra::extract::Query`) instead of `serde_urlencoded`.
/// This supports repeated fields (`field=a&field=b`) deserializing
/// into `Vec<String>`, which the standard `axum::Form` does not.
pub(crate) struct HtmlForm<T>(pub T);

impl<T, S> FromRequest<S> for HtmlForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let value = serde_html_form::from_bytes(&bytes).map_err(|e| {
            tracing::warn!(?e, "HtmlForm deserialization failed");
            StatusCode::BAD_REQUEST
        })?;
        Ok(HtmlForm(value))
    }
}
